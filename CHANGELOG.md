# Changelog

One `## vX.Y.Z` section per release, newest first, with `## Unreleased` on top. Inside a
section the subsections are, in this order and only when non-empty: `### Breaking`,
`### Added`, `### Changed`, `### Fixed`. Every user-visible change lands here in the PR
that makes it, written for a user, not a contributor: omit demo alignment, CI wiring,
review history, and other maintainer-only work. On release the version's section becomes
the GitHub release notes verbatim.

## Unreleased

### Breaking

- Existing `ready` tasks migrate to the new `open` inbox when a v3 store is first saved as v4. `ctrl+n` no longer opens the Notes editor, and `ctrl+o` now sets a task to `open` instead of reopening it to `ready`.

### Added

- Added `tsk help <command>` as an alias for each command's `--help`, and `tsk --version` / `tsk -V` for the installed version.
- Added the `open` status for captured and untriaged tasks, an expandable inbox group under ON DECK, and explicit `ctrl+n` (ready) and `ctrl+o` (open) board verbs.
- Added `tsk status T<n> open`, plus `tsk list --open` and `tsk list --ready` filters.
- The Projects Overview now previews the cursored project beside the index at wide widths, opening the Split preview when you click or move the index selection. It has a live narrow project board at the Rail stage and no full-screen task stage. The right board names the selected project in its top row and keeps its own selection and editing state.
- Direct `tsk list T<n>` output now includes the task's notes, steps, and thread in both human-readable and JSON forms, so agents can read the complete task with one command. Human detail separates notes, steps, and the trailing thread into blocks; step short IDs remain JSON-only. All human `tsk list` content wraps to the terminal width with hanging indentation, supported from 50 columns.

### Changed

- The starter tour mentions the wide projects preview and `tsk help`; the upgrade notice for this release covers the inbox, the projects preview, and the CLI reference.
- Task-page and expanded quick-add focus now share a title-entry-only Tab ring, task-page footers show thread before scope, and inbox headings use normal text.
- The agent skill (`tsk guide`, `tsk setup <agent>`) is slimmed to rules of engagement, board language, the exit contract, and workflows; flag tables and JSON shapes now live in `tsk help <command>` and the CLI guide. It gains a `Refine a task` section: a shaping pass that reads the task and its neighbours, grounds in code, proposes, and only then writes the rewrite back with `tsk edit` or `tsk add`. Skill version 1.2.0; rerun `tsk setup` to update installed copies.
- Task-verb refusals print as `code: message` on stderr for every command: `archive` and `unarchive` gain the stable `unknown-task` and `soft-deleted-task` codes, and `status`, `edit`, and `steps` gain a human message after theirs.
- Command help now uses one 80-column reference skeleton with usage, options, examples, refusals, and exits; detailed retry and output rules remain in the CLI guide.
- `ctrl+s` now starts both open and ready tasks. The bundled agent skill is version 1.2.0 and documents the inbox and status filters.
- Help is now a searchable shortcut reference with a focused search field, dim divider, and one binding per row grouped by function; long descriptions wrap beneath their description column. It stays within half the terminal height except below 15 rows, where it may grow to six rows to keep a result visible. Press `?` from any non-text surface; typing filters by key or action, and `Esc` clears the search before closing.
- The bundled agent skill now uses JSON as the default contract for every `tsk list` read. Human listings are reserved for output shown to the user or interactive troubleshooting.

### Fixed

- `tsk setup agents --yes --force` rewrites every detected agent skill even when the installed version already matches; before, `--force` only worked with a single named agent.
- `tsk setup herdr` no longer prints the content-addressed plugin root on success (it stays in error messages), and says `previous registration at <path> is gone, re-registering` when the root Herdr had registered no longer exists.
- Selected task-page footer controls no longer retain dim styling.
- Expanded quick-add in a project preview now keeps its draft title in the right-column header and renders the active notes line with the editor's focus style.

## v0.8.2

### Breaking

- Task store format 3, migrated automatically on first open. Rolling back to 0.7.x afterwards refuses the file; restore the `tsk.json.v2` backup written beside it if you need to.

### Added

- Starter tour on first board open: four desk tasks (`N1` to `N4`) that teach the tabs, sections, status keys, the task page, and the CLI, each cleared for good with `ctrl+d`, `ctrl+f`, or `ctrl+x`. Agents never see them: `tsk list` hides `N` rows.
- What's new on your desk: after an upgrade, one `N` row can summarise the release with a link to the changelog. This release carries none, so a 0.7.x upgrader sees only the tour.
- `tsk update` upgrades installer-managed copies in place; Homebrew copies print `brew update && brew upgrade tsk`. The board nudges once a day when a newer release is out (`TSK_NO_UPDATE_CHECK` disables).
- `tsk setup` detects the coding agents on your machine and offers to install the tsk skill for each; `tsk setup agents --yes` does it unattended. The curl installer asks the same question once when Herdr is on PATH.
- Added support for OMP with `tsk setup omp`, thanks @bnivanov.
- Outside Git, the current directory is available as a project: `2` opens its board, quick-add files there once it is open. The desk stays the default for capture and the CLI.

### Changed

- The agent skill (`tsk guide`, `tsk setup <agent>`) is rewritten around a quick-reference table and rules of engagement: agents hand work back with `review` and leave `done` to you. Skill version 1.1.0; rerun `tsk setup` to update installed copies.
- Sections hold their order while you work: NEEDS YOU, IN MOTION, DONE and the drawer's ARCHIVED group keep the most recent status change on top, and ON DECK lists its backlog oldest first, `N` rows leading. Editing a task or ticking a step no longer jumps it to the top.

### Fixed

- On a wide board, clicking a task opens its details beside the board and keeps board focus; a double-click during column reflow opens the task you clicked, not a newly exposed control.
- `prefix+t` opens or focuses one board per Herdr workspace, across tabs, and returns to the tab you pressed it from.
- `tsk setup herdr` on a Herdr older than 0.9 says so (`herdr 0.6.8 found; tsk needs 0.9.0 or newer`) and stops before touching the config, instead of failing on `herdr config check` with a usage dump.
- Setup errors that quote Herdr's output keep their line breaks instead of printing a literal `\u{000a}`.

## v0.7.0

### Added

- `tsk guide` prints the agent workflow, and `tsk setup <claude|pi|cursor|grok|codex|opencode|--skill-dir>` installs it as a skill. `tsk --help` ends with a pointer for agents.
- The installer adds `~/.local/bin` to PATH for Bash and Zsh.

### Fixed

- At 110 columns or wider, `Tab` on the quick-add line opened a draft page that was never painted.
- A reopen request replaced by another of the same size in the same instant could be delivered as the first one.

## v0.6.0

### Added

- Native macOS and Linux binaries, a checksum-verifying install script, and a Homebrew formula.
- `tsk setup herdr` registers the installed binary with Herdr and adds `prefix+t` (board) and `prefix+a` (capture).
- CLI: `tsk status`, `tsk edit`, and `tsk steps rename|remove`.
- **NEEDS YOU** section: blocked and review tasks at the top of the board.
- Projects index with search, per-project status counts, and persistent desk · project · projects navigation.
- Quick capture (`tsk capture`) opens the task editor in a Herdr popup; the expanded quick-add takes threads and steps.

## v0.5.0

### Added

- Wide layout: at 110 columns or wider the board and task page form a four-stage slider driven by `←` / `→`.
- Archive for tasks (`ctrl+f`) and projects (from the `p` picker), with a collapsible archived group in the done drawer and read-only focus for archived projects. CLI: `tsk archive`, `tsk unarchive`, `tsk project archive|unarchive`, `tsk list --archived`.
- Trash: deleted tasks move to `trash.jsonl` once undo can no longer reach them and purge after 30 days. `tsk list --deleted` and `tsk trash restore T<n>`.

### Changed

- State files are private on Unix: `~/.tsk` is `0700` and its files `0600`; symlinked state roots are refused.
- Store format 2: a `projects` map; a v1 store migrates on load and keeps a `tsk.json.v1` backup.
- Undo history is capped at 50 entries.
- License is MIT.

## v0.4.0

- Markdown in notes (view and peek): bold, emphasis, code, fenced blocks, headings, lists, in mono modifiers only.
- Mouse text selection: drag to highlight, release to copy via OSC 52.
- Board and task-page scrollbars with click and drag; the current section header pins under the tabs.
- Modal overlays for help, palette, and project picker share one centered card.

## v0.3.0

- Threads: an optional normalized label per task, set with `--thread` on add or in plan JSON, filtered with `list --thread`.

## v0.2.0

- Steps: an ordered checklist on the task page, with `steps <task> add|toggle` on the CLI. Step progress never changes task status.

## v0.1.0

- Queue board for capture and human status (`ready`, `started`, `blocked`, `review`, `done`) inside Herdr, with peek, task page, project scope, done drawer, and undo.
