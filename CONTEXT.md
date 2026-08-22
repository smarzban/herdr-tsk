# Glossary

Canonical terms for this repo. No implementation detail.

- **add command**: headless `herdr-tasks` subcommand that creates tasks without opening a TUI.
- **list command**: headless `herdr-tasks` subcommand that prints tasks from the board store without changing them.
- **bulk add**: one `add` invocation that accepts many items, reports each, and makes one durable write of the successes.
- **invocation default**: capture scope used when the caller omits `project` / `-p`. The cwd repo when known, otherwise global.
- **tiny result**: JSON report from a plan-shaped `add`. `created` and `existing` rows carry index, id, title. `failed` rows carry index, title (string or null), error code, and human error text. Notes are not echoed.
- **exit contract**: `add`/`list` exit `0` means every item was created or already existed (or list succeeded). `1` means one or more item refusals: retry only the failed subset. `2` means usage or parse error: nothing persisted. `3` means store I/O: commit is indeterminate; verify with `list` and retry only what is missing.
- **error code**: stable machine-matchable token on a failed item (`empty-title`, `invalid-title`, `invalid-item`). Any C0 control makes a title `invalid-title` before trimming, including a control-only title. The human `error` string is not a contract.
- **persisted**: a later `list` against the same store returns that task.
- **nothing persisted**: a later `list` against the same store shows no task created by that invocation.
- **listed**: the task appears in `list` because it is not soft-deleted.
- **exactly once**: one `list` row per task id.
- **closed global-flag set**: `--find-board-pane` and `--help`. Recognized only before the first positional argument. Nothing else selects a surface.
- **stdin_is_tty seam**: injected boolean the headless parser uses instead of asking the live process whether stdin is a TTY.
- **checklist**: the flat ordered list of items attached to one task. Exactly one level deep; there is no nesting.
- **checklist item**: a single checklist entry: stable identity, one line of text, a done flag. Nothing else.
- **item done**: an item's checked state. Progress information only; it never implies, drives, or auto-applies a task status change.
- **toggle**: flip one item's done flag. The only checklist state change that is not a text change.
- **item cursor**: the task page's checklist-row focus. Inactive on page open; a bare ↓ activates it (again after any deactivation), ↑ from the first item deactivates it. While active, bare arrows move it and modifier-protected verbs act on it; a single click on an item row moves it.
- **item short id**: an unambiguous prefix of an item's stable identity, the way tasks are addressed from the CLI.
