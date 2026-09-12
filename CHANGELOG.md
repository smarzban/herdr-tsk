# Changelog

One `## vX.Y.Z` section per release, newest first, with `## Unreleased` on top. Inside a
section the subsections are, in this order and only when non-empty: `### Breaking`,
`### Added`, `### Changed`, `### Fixed`. Every user-visible change lands here in the PR
that makes it, written for a user, not a contributor: omit demo alignment, CI wiring,
review history, and other maintainer-only work. On release the version's section becomes
the GitHub release notes verbatim.

## Unreleased

### Fixed

- `tsk setup herdr` on a Herdr older than 0.9 says so (`herdr 0.6.8 found; tsk needs 0.9.0 or newer`) and stops before touching the config, instead of failing on `herdr config check` with a usage dump.
- Setup errors that quote Herdr's output keep their line breaks instead of printing a literal `\u{000a}`.

## v0.8.1

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

### Fixed

- On a wide board, clicking a task opens its details beside the board and keeps board focus; a double-click during column reflow opens the task you clicked, not a newly exposed control.
- `prefix+t` opens or focuses one board per Herdr workspace, across tabs, and returns to the tab you pressed it from.

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
