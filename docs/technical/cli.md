# CLI

**Responsibility.** Headless `add`, `list`, and `steps`: parse argv, optionally
read a JSON plan, mutate or read the store under the lock, present text or JSON,
return an exit code. No TUI.

**Public surface.** `cli::run_with`, `CliOutput`, submodules `router`, `parser`,
`add`, `list`, `steps`, `presenter`. Process routing also exposes `router::route` /
`Surface` for `main`.

## How it works

`run_with(args, stdin, stdin_is_tty)` switches on `args[1]`. Stdin is consumed
only by plan-form `add`. `stdin_is_tty` is injected so tests do not ask the live
process (glossary: `stdin_is_tty seam`).

### Router

`route` is pure. Closed global-flag set before the first positional:
`--find-board-pane`, `--help`. Two global flags, or a global flag plus anything
else, is `Usage`. Subcommand wins over `TSK_MODE`. Unknown positional is `Usage`,
not board.

### `add`

Flag form (`-t` / `--title` etc. sets `has_item_flags`): require a title, refuse
C0 before trim (`invalid-title`), refuse empty after trim (`empty-title`). Under
`locked_transition_if_changed`, resolve scope (`resolve_flag_scope`), look for a
non-soft-deleted match on **title + scope + thread**, return `Existing` without
writing, or `create_with_thread` with `ProvenanceOrigin::Capture`, no capsule, no
agent meta.

Plan form: `--file path`, `--file -`, or piped stdin when there are no item flags
and stdin is not a TTY. JSON must be an **array**. Each object is validated first;
valid items are applied in one durable write. Failures stay in `failed` with
index `i`. Exit 1 if any failed, 0 if all created or existing.

`--desk` cannot combine with `--project`. Item flags cannot combine with `--file`.
Piped stdin with item flags is ignored (not read).

### `list`

Read-only `store.load()`. Default view: open tasks (ready/started/blocked/review)
in the invocation project, or desk outside a repo. `--all` every scope; `--desk`
desk only; `--done` / `--deleted` exclusive. `--thread` normalizes at argv
(invalid name → usage exit 2). A positional UUID lists that one task and its
steps; it cannot combine with scope/thread/status filters. Unknown well-formed
UUID is usage (exit 2), not store failure.

Human output groups STARTED / READY / BLOCKED / REVIEW (or DONE / DELETED).
`--all` groups by status then project, with a concise trailing path or `desk`.
JSON: flat array; single-task listing adds `steps` on the row.

### `steps`

Positionals: task id, `add` `<text>` or `toggle` `<short-id>`. Flags may appear
anywhere. Refusals resolve **under the lock** and set `changed = false` so nothing
is written. Toggle of a prefix that matches two steps is `ambiguous-step`. Empty
short id matches nothing. Blind retry of a successful toggle flips it back —
help text tells the caller to `list` first.

Short ids are the shortest unambiguous prefix of the step UUID among siblings
(`step_short_id`).

### Presenter

Maps results to `CliOutput`. Store errors → exit 3. Item refusals → exit 1 with
the stable `code()` token on stderr for flag add/steps. Plan add prints the JSON
object even when `failed` is non-empty (exit 1). Untrusted titles pass through
`terminal_text` on human list output.

## Invariants

[Invariants](invariants.md) §§31–33. Idempotency includes thread (ADR-0002). C0
in titles and step text is `invalid-*` before trim. `--state-dir` overrides the
directory for that invocation only.

## Error paths

| Code | Meaning |
| --- | --- |
| 0 | listed / created-or-existing / step applied |
| 1 | item refusal (retry the failed subset; for toggle, verify first) |
| 2 | usage/parse; nothing persisted |
| 3 | store I/O; commit indeterminate |

## Extension points

New flags go in the relevant parser with an explicit combination check. New JSON
fields on plan items need `parse_plan_item` and a refusal code. Do not grow the
global-flag set without updating `route` tests and `CONTEXT.md`.
