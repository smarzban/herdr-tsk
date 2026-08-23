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
- **steps**: the flat ordered list of step entries attached to one task. Exactly one level deep; there is no nesting.
- **step**: a single step entry: stable identity, one line of text, a done flag. Nothing else.
- **step done**: a step's checked state. Progress information only; it never implies, drives, or auto-applies a task status change.
- **toggle**: flip one step's done flag. The only step state change that is not a text change.
- **step cursor**: the task page's step-row focus. Inactive on page open; a bare ↓ activates it (again after any deactivation), ↑ from the first step deactivates it. While active, bare arrows move it and modifier-protected verbs act on it; a single click on a step row moves it.
- **step short id**: an unambiguous prefix of a step's stable identity, the way tasks are addressed from the CLI.
- **thread**: an optional single name a task carries, grouping it with same-named tasks in the same scope. Not an entity: no identity beyond the name, no status, no lifecycle. It presents on the board exactly while at least one open task in its scope carries it; the field itself persists on the task regardless of status.
- **thread token**: the `!t name` capture directive, sibling of `!p`; bare `!t` means unthreaded. Stripped from the saved title.
- **thread header**: a derived, non-selectable board row above a thread's open tasks in a scoped deck group, showing the thread name and open count. Chrome, not a task.
- **unthreaded**: carrying no thread. Unthreaded tasks list after thread groups within their deck group.
- **open task**: human status ready, blocked, or review, and not soft-deleted. The tasks a thread header groups and counts.
- **thread block**: the derived unit of one thread's header plus its ordered open tasks inside a deck section. Exists only in query output, never in the store.
