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
park/resume, linking, and dispatch are not in this tree; reference lives on
`archive/dark-engine-pre-v1`. Crate (`tsk-tui`) and plugin are `0.7.0`.

A gitignored `HANDOFF.md` may hold this clone’s live working state.

For scriptable board work, use `tsk add`, `tsk list`, `tsk status`, `tsk edit`, and
`tsk steps`; read `skills/tsk-cli/SKILL.md` first for retry and scope-check rules.

### Board

- Selected tasks use a `▸` selection arrow beside the status glyph (`●` for Started) and number, with no filled highlight; active tabs are bright and underlined, inactive tabs dim.

- The board keeps persistent destinations **desk** · **selected project** · **projects**
  (`1`/`2`/`3`) in every normal board surface. `p` opens the project picker; selecting a
  project fills slot 2 and opens its board. **desk**: NEEDS YOU across live scopes, global
  IN MOTION, then desk-only ON DECK. **projects**: one selectable overview row per project,
  with right-anchored needs-you, in-motion, and ready counts (zero paints a dim `·`, live
  needs-you bold), a blank row under the legend, basename-only rows with a dim `here` on the
  launch project, a THREADS column at ≥100 columns, and the selected row's full path on the
  status row. Click selects an index row, double-click or Enter opens it. Its View can show a
  flat cross-project thread board. Project focus remains available through `p` and the index. NEEDS YOU · IN MOTION ·
  ON DECK when project-scoped · done drawer (`d`). Project rows are navigation, never task
  rows. Scoped project boards show thread labels in task peeks and a local thread filter,
  without a separate filtered-task count. Cross-project thread views also omit the task/project
  summary. A selector wrapped below the tabs has one blank row above it.
  The done drawer ends with a collapsible `▾ archived · n` group (closed by default,
  session-only): its header is the one selectable header (`Enter`/click toggles it, the word
  paints bold when selected, never reverse), its rows paint dim, and it folds with bare `g`
  while the drawer is open; other board surfaces have no collapsible groups. Visible tab labels are `desk` ·
  selected project · `projects`; shortcuts `1`/`2`/`3` remain keyboard-only. Projects Overview
  search opens in the shared footer slot with `/`, keeps the table visible, filters live, opens
  the selected project on Enter, and clears/closes on Esc; the closed footer advertises `/ search`.
  Collapsed task rows show no attribution. Open peek shows project attribution on global
  rows or `#thread` on project rows on a dim `└─ label` line below its notes, never relative ages.
  Titles wrap two cells before the right edge; unlabeled tasks have no attribution line; task-page informational dates remain intact.
- Archive is a flag, not a place. `archived` on a task, and a lazy project record
  (`projects` map keyed by scope path, present only while archived) for projects. Neither leaves
  `tsk.json`. A *hidden* task (archived, or in an archived project) paints in no working lens;
  `open task` excludes archived. `ctrl+f` ("file") toggles the selected task; in the `p` picker
  it archives the selected project, on the picker's archived tab it unarchives. `ctrl+u` on an
  archived selection unarchives, otherwise it is undo. No undo entry for archive.
- The `p` picker has two tabs (main · archived) with a dim rule under them. `Enter` on an
  archived project opens a **read-only focus**: chip `name · archived`, dim rows, every
  mutating verb refuses with `project <name> is archived · ctrl+u unarchive`, task page is
  view-only, `ctrl+u` unarchives in place, `Esc`/`p`/`1`-`3` leave. The only lens that paints an
  archived project's tasks. Scope dropdowns (task page, quick-add, capture) never offer an
  archived project. Launching inside an archived project's directory shows a once-per-session
  card (`project <name> is archived, would you like to unarchive it?`, `y`/`n`/`Esc`); keep →
  quick-add defaults to the desk for the session. `!p name` to an archived project refuses.
- Standard ≥78×24, compact below, operable to 40×10. At 110 usable columns or wider the
  board is a four-stage slider, and focus is the stage: **0** board full width · **A** board
  `floor(w*0.4)` beside the task page (board focus) · **G** a dim 32-column rail beside the
  page (task focus) · **F** page full width. One dim `│` rule column and one pad cell sit
  between the columns; no box, no border, no colour anywhere. Bare `→` / `←` slide the stage,
  `Enter` opens F and remembers the stage it left, `Esc` from F returns there and from G parks
  the page beside the board. `→` never peeks at wide widths. The session opens in stage 0; the
  stage is never persisted and survives resizes (0/A render the single board below 110, G/F
  the single task page). Exactly one footer spans the frame: rule, status row (with a dim
  stage crumb on the right), verb bar following focus. The task column paints its header on
  the selector row — `● T12 title … started · tsk` with the status glyph restored, DIM in A
  and BOLD in G/F, the slot reading `editing <field>` or `unsaved` — and a dim dash rule on
  the row under it, instead of the in-page header. A click on the left side takes focus
  left: a rail row click selects the row and lands the board beside it. Starting a field edit
  from the board side hands focus to the task: from stage 0 it jumps straight to F (origin
  recorded), from A it moves to G. Below 110, nothing changes: the focused surface fills the
  frame.
  Persisted task rows paint a dim `T<number>` prefix before the title, including task page
  and done drawer; clicking that prefix copies it. Peek relies on its parent row's prefix.
  Drafts without a number paint none.
- Human status: `ready` · `started` · `blocked` · `review` · `done`.
- Mutating verbs (`s` `d` `o` `b` `r` `e` `n` `x` `u` `f` `q`) need Ctrl. Bare
  letters do nothing unless they carry a bare route. Nav, peek, `Enter`, `p` (picker),
  `d` (done drawer), `g` (archived group), `t`, `v`, `1`/`2`/`3`, `:`, `?`, `+`, and `Esc`
  stay bare; the keymap resolves one letter by modifier class (`d` drawer vs `ctrl+d` done).
  `ctrl+r` toggles review ↔ ready.
- The footer's verb row is a prompt, not a keymap: open · status verbs · `+ add` · `? help`
  on the board, `ctrl+e edit` · status verbs · `esc close` on the page, `shift+enter save` ·
  `esc cancel` while editing. A visible delete notice prefixes the board prompt with `ctrl+u undo`.
  Archive, delete, drawer, pickers, and palette are not seats; `?` lists every key of every surface on one scrollable card (`↑↓`/`jk`/page/wheel,
  closes on `Esc`/`?`/`q`). Keys paint as `ctrl+x label`; inputs' placeholders hold only the
  content hint, and the reserved row above a bottom input carries its context (quick-add
  destination, selected project path).
- Task creation is the quick-add bar, never a form takeover. `+` opens a one-line
  title input on the status-row slot with a blank row above and below, list still
  visible. `Enter` saves and closes, `Shift+Enter` saves and stays open, `Tab`
  expands the draft onto the task page with a title·notes·thread·scope stash and a
  `+ step` row, so Esc returns to the line and a second `Tab` restores what was typed. The
  expanded draft owns the whole frame at every width (`capture_draft_open`): the wide slider
  has no column for a non-task form, so it never paints beside the board. A project board
  defaults the draft to that project, a project-less board defaults it to your desk, and home
  keeps the invocation cwd-derived default. Capture tokens in the title: `!p` and
  `!t` each consume one whitespace-delimited argument. Bare `!p` selects your desk,
  `!p name` selects a project basename (case-insensitive), and `!p /path` uses that
  path verbatim. Bare `!t` unthreads, while `!t name` assigns a normalized thread:
  lowercase ASCII alphanumerics, hyphens, and dots, starting with an alphanumeric, at most
  32 characters. A refusal names the rule (start character, allowed characters, or length). Tokens are stripped from the saved title. A saved task becomes the
  selection. Success has no status message: the row flash is the feedback. Refusals
  paint while the line is open and clear when it closes.
- There is no inline board capture form and no standalone capture-form surface.
  Creation detail lives on the task page. Quick capture (`tsk capture`,
  or `TSK_MODE=capture`) runs the same board session seeded onto the expanded
  quick-add page (title focused, launch card skipped): a persisted save or an
  explicit discard exits the process (which closes the host popup), and a failed
  save keeps the draft editable through the usual recovery. The host launcher opens
  it as a fixed-size popup below the wide threshold (`scripts/open-capture.sh`);
  the legacy Capture form module (`src/ui/capture.rs`) is no longer reachable from
  the binary.
- Below 110 columns, row click peeks, clicking the same row again closes peek, and
  a fast double-click opens the page. At wide widths there is no peek: a board or rail
  row click selects it in place (a rail click lands the board in stage A), and a fast
  double-click opens the full task page.
- Task page is view-first. Only `ctrl+e`, `ctrl+n`, or bare Tab enter edit mode,
  the one exception being a quick-add draft expanded with `Tab`, which opens
  straight into Notes edit mode because a draft has nothing to view. The status verbs
  (`ctrl+s` `ctrl+d` `ctrl+o` `ctrl+b` `ctrl+r`) always act on the task; `Enter` on a stored
  step toggles it (`ToggleStep`, resolved at the keyboard boundary in `src/app.rs` so the
  save baseline loads first). Plain `Enter` parks an existing-step rename; on a new step it
  saves that step and opens the next empty row; in the Title editor it moves to Notes.
  `Shift+Enter` is the only whole-session save chord (`Alt+Enter` does nothing on the board;
  standalone Capture keeps its own fallback). An empty new-step row discards if you click
  elsewhere. `ctrl+x` asks once (`press ctrl+x again to delete`) before deleting a task. The scope footer is
  inert until an edit has started.
- Content and chrome use mono modifiers only, at every width. No color theme module.
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

- `tsk setup herdr` explicitly registers embedded plugin assets from `src/setup.rs`,
  sharing the stable invoked executable path (not a canonicalized Homebrew Cellar path).
  It adds prefix+t / prefix+a with conflict confirmation, aborting before writes for
  noninteractive conflicts. Re-running setup does not duplicate bindings. Test setup
  with isolated XDG config/state roots AND HERDR_SOCKET_PATH: config override alone
  does not isolate Herdr's plugin registry. Never smoke setup against a daily plugin.

- Native packaging lives in `scripts/release.py`, `site/public/install.sh`, and
  `.github/workflows/release.yml`; see `packaging/README.md`. Python 3.11+ packaging
  checks: `python3 -m unittest discover -s tests/packaging`. The release workflow is
  manually dispatched against an existing stable tag and only creates a draft.
  Installer payloads come from published release assets, never main. Homebrew uses
  a generated, version-pinned formula for `smarzban/homebrew-tap`; formula updates
  are separate from release publication. Installer-only changes run the packaging
  tests and ShellCheck via `.github/workflows/installer.yml`; Rust CI also exercises
  binary-dependent packaging/PTY tests after building the release executable.
- The install script appends a duplicate-safe PATH entry for Bash (`.bashrc` plus
  the first existing login profile, default `.profile`) or Zsh (`$ZDOTDIR/.zshrc`,
  otherwise `~/.zshrc`) only when the install directory is absent from PATH.
  It never sources startup files; symlinked, non-regular, or unwritable files are
  skipped with manual guidance. Partial success preserves completed edits and names
  skipped files. Successful setup prints reopen-terminal/export instructions.
  Tests must isolate HOME and ZDOTDIR, never edit the caller's startup files.
  Website installer updates do not replace an already-published release's assets.

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
  Warm, it takes about three minutes (`cargo test` alone runs 1000+ tests in under 30 s). If
  it takes 20 minutes, a second cargo process is holding the `target/` lock: cargo waits
  silently on it for every one of the 40+ test binaries. Never run two cargo commands in this
  checkout at once, including a background release build or another agent's run. Run the bar
  once at the end, as a single chain, not after every edit.
- CI installs Rust **1.96.0** with `rustfmt` + `clippy` on ubuntu and macos. The
  frame-time bench is Linux-only. In this public repository, Rust CI and Site validation
  run on pushes to `main` and pull requests targeting `main`, with the same path filters
  for both events. Site-only changes skip the Rust matrix (`paths-ignore: site/**`).
  Validation uses read-only permissions and no repository secrets, including on fork PRs.
  Vercel production deployment runs only on pushes to `main`.
  Site CI is `npm ci && npm test && npm run build` in `site/`.
- `site/vercel.json` is validated by Vercel's own route parser in `npm test`
  (`@vercel/routing-utils`, the same check `vercel deploy` runs before upload). Its
  `:param(...)` patterns reject regex lookaheads; a bad pattern fails every production
  deploy silently from the site's point of view, so never edit that file without the test.
  Markdown twins are reached by `.md` URLs and `rel=alternate`, not `Accept` negotiation.
- Landing page and Starlight docs live in `site/` (Astro). They are not part of
  the `tsk` binary. Production: https://gettsk.sh. Point Vercel at
  this repo with Root Directory `site`.
- The docs and the website ship with the feature. Any change that adds, removes, or
  alters user-visible behaviour (a key, verb, palette command, mouse target, status
  message, CLI flag or output, exit code, board section, file or env var) lands in the
  same PR as its `site/` update: the relevant page under
  `site/src/content/docs/docs/`, the landing page (`site/src/pages/index.astro`) and
  demo (`site/public/board-demo.js`) when they show it, `site/public/llms.txt`, and
  the README when the claim lives there. Remove docs for what you remove. The docs
  describe only what ships; upcoming work is marked as such, never as present.
  Before calling a docs pass done, diff the guide against `src/` (keymaps in
  `src/ui/input.rs`, palette in `src/ui/board/commands.rs`, CLI in `src/cli/`).
  The web demo uses bare verb keys on purpose (browsers reserve control chords); do
  not "fix" that to match the TUI.

### Live herdr smoke

When the change touches the board, host integration, or panes, do not call it
done on unit tests alone.

If `HERDR_ENV=1`: rebuild in-repo, drive the real path with herdr, read the pane,
and fix anything that only fails live. Every UI-affecting addition or behavior change needs a
real UI smoke of its new flow, using isolated state when it would mutate a user's tasks; unit
and render tests alone are not enough.

If `HERDR_ENV` is unset, say that live smoke was not run.

## Conventions

- Invocation scope is the nearest Git repo root, or the current directory when outside
  Git. This applies to board launch/reopen, capture, and CLI defaults. Desk remains an
  destination (`1` for navigation; `--desk` or bare `!p` for capture); existing task
  scopes never move.

- Human status is source of truth. Never auto-complete tasks from agent status.
- The projectless scope displays as `desk` but serializes as `global` in tsk.json and
  keeps its internal name `TaskScope::Global`; do not "fix" either without a store
  migration.
- State is `$HOME/.tsk/tsk.json`, walkthrough dismissal is
  `$HOME/.tsk/walkthrough.json`, the release-check cache is `$HOME/.tsk/update.json`,
  the notice delivery record is `$HOME/.tsk/delivery.json`,
  overridable with `TSK_STATE_DIR` / `TSK_CONFIG_DIR`.
  Host-injected `HERDR_PLUGIN_*` dirs are ignored. Mutating verbs always use Ctrl.
  On board launch, if `TSK_NO_UPDATE_CHECK` is unset and that cache is older than 24h,
  a background `curl` of the GitHub latest-release tag updates it; a newer tag paints a
  dim `v<tag> available` on the idle status row. Any check failure is silent.
- Starter tasks (`src/guides.rs`, catalog ids `guide.*`) are four desk notice tasks seeded once
  per state dir by the full board open only (`load_board`, never quick capture, the CLI, or
  the installer). `delivery.json` (`src/delivery.rs`) holds the delivered or dismissed catalog
  ids and the announcement watermark; the seeder dedupes by catalog id in the store, so a
  lost record converges without duplicates. `ctrl+d`, `ctrl+f`, and `ctrl+x` on a notice
  share one dismiss rule: the persist boundary marks its catalog id and it never re-seeds.
  Seed and dismiss are silent; neither writes `status_message`.
- Release announcements (`src/announcements.rs`) are the maintainer-edited
  `src/announcements/catalog.toml`, compiled in with `include_str!`: `[[announcement]]` tables
  with exactly `id`, `title`, `notes`; ids positive and increasing in file order; the
  changelog link lives in `notes`, never in `title` (the parser refuses otherwise). Append one
  entry per release worth a row. `seed_on_open` runs right after the guide seed on the full
  board open: it takes the higher of `delivery.announcement_watermark` and any `announce.<id>`
  notice in the store as "seen". A blank state checked before guide seeding (watermark 0, no
  delivered guide ids, no live tasks) is a fresh install: jump the watermark to the bundled
  maximum and create nothing. Otherwise every entry above seen becomes one desk notice
  `What's new in tsk` (catalog id `announce.<max>`, notes newest first) and the watermark
  moves to the maximum. That includes upgrading from a guides-only install whose watermark
  is still 0.
- Store format is versioned (`STORE_FORMAT_VERSION`, currently 3). Any schema change bumps it
  and adds a `vN → vN+1` step to `MIGRATIONS` in `src/store.rs`; `deny_unknown_fields` stays on
  `Task` and `DomainState` so an older binary refuses a newer file instead of dropping fields.
  A lower version loads migrated in memory and the first save writes `tsk.json.v<N>` beside the
  live file (never overwritten). Higher or missing versions are refused.
- Deleted tasks leave `tsk.json` for `trash.jsonl` once undo can no longer reach them (or after
  7 days) and are purged 30 days after deletion. The trash is rewritten atomically, readers
  dedupe by id and skip torn lines, and trash is always durable before the live document loses
  a task. `tsk list --deleted` reads it; `tsk trash restore T<n>` brings a task back. Nothing on
  the board reads trash.
- CLI verbs beyond add/list/steps: `tsk status T<n> <status>`, `tsk edit T<n>`, `tsk trash restore`, `tsk archive|unarchive T<n>`,
  `tsk project archive|unarchive <name>`, `tsk list --archived`; `tsk add` into an archived
  project refuses with error code `project-archived`. `tsk status` accepts ready, started
  (or start), blocked, review, or done. `tsk steps` also renames and removes.
  `tsk guide` prints the embedded `skills/tsk-cli/SKILL.md` body (also published at
  `/docs/agents/`, generated at site build). `tsk setup <claude|pi|cursor|grok|codex|opencode|--skill-dir p>`
  writes that skill into the agent's skills dir (`src/setup_agent.rs`); bare `tsk setup` lists
  and writes nothing. `tsk --help` ends with a two-line agent footer.
- `~/.tsk` must live on a local disk (flock plus rename-based replace); synced folders are
  unsupported, `TSK_STATE_DIR` is the escape hatch. State/config directory roots must be real
  directories, not symlinks; permission hardening refuses a symlink instead of chmodding its target.
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
