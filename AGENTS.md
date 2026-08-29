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

**tsk** is a terminal task board ("a task board for your terminal"): a queue
board for capture and human-status verbs. It ships as the herdr plugin
`herdr-tsk`, and the built binary (`tsk`) also runs standalone. Attention,
park/resume, linking, and dispatch are not in this tree; local reference only
on `archive/dark-engine-pre-v1`. Crate (`tsk-tui`) and plugin are `0.3.0`.

A gitignored `HANDOFF.md` may hold this clone’s live working state.

For scriptable board work, use `tsk add` and `tsk list`; read
`skills/tsk-cli/SKILL.md` first for retry and scope-check rules.

### Board

- Home is a tabbed board: **desk** · **projects** · **threads** (`1`/`2`/`3`). Tabs
  show only at home; project focus (`P` → project) hides them and paints the project
  name chip (`P ▾` on the right). At home the chip is hidden — tabs already say where
  you are; `P` still opens the scope picker from the keyboard. **desk**: global IN MOTION
  plus desk/global ON DECK. **projects**: one collapsible group per project (single-click
  collapse, double-click → project focus); in motion under each project, then review,
  blocked, open. **threads**: cross-project thread names with collapsible project
  sub-groups; same status order. Collapse state is session-only. IN MOTION · ON DECK when
  project-scoped · done drawer (`z`). Scoped ON DECK thread blocks paint dim `#name`
  headers with open counts; headers consume row budget but are not selectable or
  hit-testable.
- Standard ≥78×24, compact below, operable to 40×10
- Human status: `ready` · `started` · `blocked` · `review` · `done`. The store
  still reads old `todo`/`doing` values.
- Mutating verbs (`space` `d` `o` `b` `e` `n` `x` `u` `q`) need Alt, or Ctrl
  if the user flipped it in the palette. Bare letters do nothing. Nav, peek,
  `Enter`, `P`, `1`/`2`/`3`, `z`, `:`, `?`, `+`, and `Esc` stay bare.
- Task creation is the quick-add bar, never a form takeover. `+` opens a one-line
  title input on the status-row slot with a blank row above and below, list still
  visible. `Enter` saves and closes, `Ctrl+Enter` saves and stays open, `Tab`
  expands the draft onto the task page with a title·notes·scope stash, so Esc
  returns to the line and a second `Tab` restores what was typed. A project board
  defaults the draft to that project, a project-less board defaults it to your desk, and home
  keeps the invocation cwd-derived default. Capture tokens in the title: `!p` and
  `!t` each consume one whitespace-delimited argument. Bare `!p` selects your desk,
  `!p name` selects a project basename (case-insensitive), and `!p /path` uses that
  path verbatim. Bare `!t` unthreads, while `!t name` assigns a normalized thread:
  lowercase ASCII alphanumerics and hyphens, starting with an alphanumeric, at most
  32 characters. Tokens are stripped from the saved title. A saved task becomes the
  selection. Success has no status message: the row flash is the feedback. Refusals
  paint while the line is open and clear when it closes.
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
- No host attention poll, park/resume, linking, or dispatch recovery on the board.
- Text wraps, never truncates: one wrap engine (`edit::wrap_text`, word-boundary,
  display-cell measured) feeds notes, task-page titles, list rows, quick-add, capture
  Notes, and peek. Task-page notes wrap at `row_width - 6` (paint capacity); the
  capped task-page header and the verb bar's tier-budget ellipsis are chrome limits,
  not task text. `escaped_draft_rows` remains only for the single-line Title/Thread
  editors.
- Notes markdown (view and peek only; edit is raw source). No color. Peek paints the
  same markers then dims every span; markdown runs on the note text, then the `│`
  gutter is prefixed. Subset: `**bold**`; `*em*` / `_em_` underline; `` `code` `` dim
  with ticks kept; `#`…`######` headings (bold+underline, dim hashes); `-` / `*` lists
  (dim bullet); fenced ` ``` ` (dim fence and body, no inline inside); `- [ ]` stays
  text (steps own checklists). Unmatched `*` and word-internal `_` stay literal.

## Build / test / verify

- Build: `cargo build --release`
- `herdr-plugin.toml` launches `./target/release/tsk`. Rebuild in-repo
  before live smoke. A running board keeps the old binary until you quit it.
- Test: `cargo test` (plain, parallel)
- `clippy::if_same_then_else` is on via `-D warnings` on the pinned 1.96.0 toolchain.
  Identical if/else bodies fail the green bar; merge the conditions.
- A regression test must fail without its fix. Write it, revert the fix, watch it
  fail, restore the fix. Use content that actually crosses the boundary under test.
- Temp state dirs need a per-binary atomic counter, not just `SystemTime::now()`.
- Green bar: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release`
- CI installs Rust **1.96.0** with `rustfmt` + `clippy` on ubuntu and macos. The
  frame-time bench is Linux-only. Site-only pushes skip that matrix
  (`paths-ignore: site/**`). Site CI is `.github/workflows/site.yml`:
  `npm ci && npm test && npm run build` in `site/`.
- Landing page and Starlight docs live in `site/` (Astro). They are not part of
  the `tsk` binary. Production: https://tsk-gules.vercel.app. Point Vercel at
  this repo with Root Directory `site`.
- Changes to the keymap, status verbs, or tab/section semantics need a matching
  `site/` update (`src/content/docs/docs/{keys,board,capture,cli}.md` and
  `public/board-demo.js`). The web demo uses bare verb keys on purpose (browsers
  steal Alt-chords); do not "fix" that to match the TUI.

### Live herdr smoke

When the change touches the board, host integration, or panes, do not call it
done on unit tests alone.

If `HERDR_ENV=1`: rebuild in-repo, drive the real path with herdr, read the pane,
and fix anything that only fails live.

If `HERDR_ENV` is unset, say that live smoke was not run.

## Conventions

- Human status is source of truth. Never auto-complete tasks from agent status.
- The projectless scope displays as `desk` but serializes as `global` in tsk.json and
  keeps its internal name `TaskScope::Global`; do not "fix" either without a store
  migration.
- State is `$HOME/.tsk/tsk.json`, config `$HOME/.tsk/settings.json`, overridable
  with `TSK_STATE_DIR` / `TSK_CONFIG_DIR`. Host-injected `HERDR_PLUGIN_*` dirs are
  ignored. Verb modifier is `settings.json`; the palette flips it.
- Golden fixtures regenerate via `cargo test --test queue_board_render regenerate_golden_fixtures -- --ignored`; never hand-edit the `.txt` files.
- Pane label matching is exact against `board_pane::BOARD_PANE_LABEL`; the manifest pane title must equal it.
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
- Selection may only ever rest on a row the current lens paints; the collapse sets
  count as visibility, so `seed_selection` and reanchor fallbacks must never pin an
  invisible task.
- `sync_from_domain` never moves the user's home tab or selection for tasks merged
  in from disk; the one exception is an otherwise-empty view surfacing the first
  arriving task (the idle-merge tests pin that visibility). A pinned save pins the
  selection only when the current lens renders the saved task; otherwise it
  reanchors near the saved task's old position.
