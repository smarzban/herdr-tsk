# tsk verification map

This directory is the maintained source for verifying user-facing tsk behavior. Read the index before driving the app, then use the matching feature file as the recipe.

## Baseline preconditions

- Build `target/release/tsk` when missing or stale (`cargo build --release`).
- Run `./.cursor/skills/verify-tsk/scripts/control-tsk init` so the store is a disposable directory, never `~/.tsk`.
- Run `control-tsk doctor` and require `"ok": true`, the expected `binary`, and a non-home `state_dir`.
- Prefer CLI recipes for store mutations. Start the board PTY only for paint and key claims.
- Never drive an instance that was not started by this verification run.

## Driving conventions

- Start every recipe from the baseline state unless its preconditions say otherwise.
- Prefer stable CLI flags (`--desk`, `--json`, `--state-dir`) and board prompt strings (`NEEDS YOU`, `IN MOTION`, `ON DECK · desk`, `/ search`) over coordinates. Do not wait for `desk` as destination proof: that word is on the tab row of every surface.
- First `board start` on a fresh store seeds notice tasks. `j`/`k` until dump shows `▸` on the intended title. `tsk list` still omits notices.
- Treat every command as literal. Keep quoted titles and flags unchanged.
- Run CLI actions through `control-tsk cli --`.
- Run board actions through `control-tsk board …`.
- Restore or discard scratch state with `cleanup`. Do not remove proof artifacts during cleanup.

## Proof and skip reporting

- Capture the user action and the resulting state, not only the final screen.
- CLI proof includes the command, stdout, stderr, and exit code (`evidence/.../cli-*.json`).
- Board proof includes a screen dump (`board-screen.txt`) and a store read when the action mutates tasks.
- Mutation proof includes a second read (`list --json` or `control-tsk store`).
- Record the feature ID and entry point used with every artifact.
- Report an unreachable path with the attempted command and the unmet precondition.
- Do not report a skipped entry point as verified through a different path.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then uses exactly four H2 sections in this order.

1. `Sub-features` lists short IDs with one line for each behavior.
2. `How to get to it (user POV)` lists every user entry point.
3. `Driving it with control-tsk` starts with `Preconditions:` and uses labeled bullets that pair each user action with an exact command and observable result.
4. `Gotchas` lists traps that can waste or invalidate a verification run.

Keep implementation details out of the map. Name only user paths, stable handles, required state, commands, and observable proof.

## Features

- [CLI add and list](./cli-add-list.md) covers desk creation, JSON list, and persistence.
- [Board status verbs](./board-status-verbs.md) covers selecting a row, starting it, and marking it done.
- [Quick add](./quick-add.md) covers the `+` capture line on the board.
- [Task page](./task-page.md) covers opening a task and toggling a step.
- [Desk and projects tabs](./desk-projects-tabs.md) covers `1` / `3` destinations and Enter on a project row.
