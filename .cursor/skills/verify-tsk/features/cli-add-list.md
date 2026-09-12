# CLI add and list

CLI add and list let a user create a desk task from the terminal and confirm it from a second list read without opening the board.

## Sub-features

- `cli-add-desk` creates a desk task with a title.
- `cli-list-desk` lists open desk tasks as JSON.
- `cli-add-idempotent` reports an existing match instead of duplicating.

## How to get to it (user POV)

- Run `tsk add -t "<title>" --desk --json` (`--json` is an output flag, not a substitute for `--desk`).
- Run `tsk list --desk --json`.
- Agents also reach the same commands via `tsk guide` / the installed CLI skill.

## Driving it with control-tsk

Preconditions:

- `control-tsk doctor` reports `"ok": true`.
- No desk task is titled `Verify CLI add`.

- **Create desk task.** Run `./.cursor/skills/verify-tsk/scripts/control-tsk cli -- add -t "Verify CLI add" --desk --json`. Exit code `0`. Stdout JSON has `"outcome":"created"`, a numeric `number`, and `"title":"Verify CLI add"` (`"project": null`).
- **List desk tasks.** Run `./.cursor/skills/verify-tsk/scripts/control-tsk cli -- list --desk --json`. Exit code `0`. Stdout is a JSON array containing one object whose `title` is `Verify CLI add` and `status` is `ready`.
- **Idempotent re-add.** Run the same add command again. Exit code `0`. Stdout JSON has `"outcome":"existing"` and the same `number`.
- **Store proof.** Run `./.cursor/skills/verify-tsk/scripts/control-tsk store`. The printed `tsk.json` contains the task title and `status` `ready`.

## Gotchas

- Outside a git repo, bare `tsk add -t` already defaults to the desk. This harness cwd is non-git on purpose; still pass `--desk` so the recipe stays explicit.
- Human list output is not stable for agents. Prefer `--json`.
- Do not point `--state-dir` at `~/.tsk`.
- A typo in `--project` silently creates a new scope. Use `--desk` for this recipe.
- An existing match still wins if the same title/scope/thread is done or individually archived. Default `list --desk` then omits that row; keep the store empty of this title.
- `tsk list` never shows notice (`N`) rows. A later board open can seed notices without changing this list.
